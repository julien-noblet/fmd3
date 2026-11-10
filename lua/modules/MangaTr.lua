function Init()
	local m = NewWebsiteModule()
	m.ID                         = 'bcf6bf0a1b5d4cffa6cc6743e29ee5f2'
	m.Category                   = 'Turkish'
	m.Name                       = 'Manga-Tr'
	m.RootURL                    = 'https://manga-tr.com'
	m.OnGetInfo                  = 'GetInfo'
	m.OnGetPageNumber            = 'GetPageNumber'
	m.OnGetNameAndLink           = 'GetNameAndLink'
	m.MaxConnectionLimit         = 8
end

function GetInfo()
	if HTTP.GET(MANGAINFO.URL) then
		local x=CreateTXQuery(HTTP.Document)
		MANGAINFO.CoverLink = x.XPathString('//img[@class="thumbnail"]/@src')
		MANGAINFO.Title     = Trim(x.XPathString('//title'):gsub('- Çevrimiçi Türkçe Manga', ''):gsub('- Çevrimiçi Türkçe Webtoon', ''):gsub('- Çevrimiçi Türkçe Novel', ''))
		MANGAINFO.Authors   = x.XPathString('//table[2]/tbody/tr[2]/td[1]')
		MANGAINFO.Artists   = x.XPathString('//table[2]/tbody/tr[2]/td[2]')
		MANGAINFO.Status    = MangaInfoStatusIfPos(x.XPathString('//table[1]/tbody/tr[2]/td[2]'), 'Devam Ediyor', 'Tamamlandı')
		MANGAINFO.Genres    = x.XPathString('//table[2]/tbody/tr[2]/td[3]')
		MANGAINFO.Summary   = x.XPathString('//div[@id="tab1"]/div[@class="well"]/text()')

		local info = x.XPathString('//*[@slug]/@slug')
		local pages = 2
		local p = 1
		while p <= pages do
			if p >= 1 then
				HTTP.Reset()
				HTTP.Headers.Values['Cache-Control'] = ' no-cache'
				HTTP.Headers.Values['X-Requested-With'] = ' XMLHttpRequest'
				HTTP.MimeType = 'application/x-www-form-urlencoded; charset=UTF-8'
				if HTTP.POST(MODULE.RootURL .. '/cek/fetch_pages_manga.php?manga_cek='..info, 'page='..p) then
					x.ParseHTML(HTTP.Document)
				else
					break
				end
			end

			if p == pages then
				local pg = x.XPathString('//*[@class="last"]/a/@data-page')
				if pg ~= '' then pages = tonumber(pg) end
			end
			x.XPathHREFAll('//tr/td[1]/a', MANGAINFO.ChapterLinks, MANGAINFO.ChapterNames)
			p = p + 1
		end
		MANGAINFO.ChapterLinks.Reverse(); MANGAINFO.ChapterNames.Reverse()
		return no_error
	else
		return net_problem
	end
end

function GetNameAndLink()
	if HTTP.GET(MODULE.RootURL .. '/manga-list.html?listType=allABC') then
		local x=CreateTXQuery(HTTP.Document)
		x.XPathHREFAll('//*[@data-toggle="mangapop"]/b/a', LINKS, NAMES)
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	TASK.PageLinks.Clear()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
		local x=CreateTXQuery(HTTP.Document)
		x.XPathStringAll('//img[@class="chapter-img"]/@src', TASK.PageLinks)
		return true
	else
		return false
	end
end
