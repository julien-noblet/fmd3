function GetInfo()
	MANGAINFO.URL=MaybeFillHost(MODULE.RootURL,URL)
	if HTTP.GET(MANGAINFO.URL) then
		x=CreateTXQuery(HTTP.Document)
		MANGAINFO.Title=x.XPathString('//h4/a/text()')
		MANGAINFO.CoverLink=MaybeFillHost(MODULE.RootURL, x.XPathString('//div[@id="cover"]//img/@src'))
		MANGAINFO.Artists = x.XPathString('//div[@id="info"]/div[contains(span, "Artist")]/a')
		MANGAINFO.Genres = x.XPathStringAll('//div[@id="info"]/div[contains(span, "Category")]/a')
		MANGAINFO.Status = MangaInfoStatusIfPos(x.XPathString('//div[@id="info"]/div[contains(span, "Status")]/a'))
		MANGAINFO.Summary=x.XPathStringAll('//div[@id="info"]/div[contains(span, "Summary")]/text()', '')
		v=x.XPath('//ul[@class="arf-list"]/li/a')
		for i=1,v.Count do
			v1=v.Get(i)
			MANGAINFO.ChapterLinks.Add(v1.GetAttribute('href'))
			MANGAINFO.ChapterNames.Add(x.XPathString('./span/text()', v1))
		end
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL,URL)) then
		x=CreateTXQuery(HTTP.Document)
		s=x.XPathString('//script[contains(.,"rff_imageList")]')
		s=GetBetween('rff_imageList =', ';', s)
		x.ParseHTML(s)
		v=x.XPath('json(*)()')
		for i=1,v.Count do
			v1=v.Get(i)
			TASK.PageLinks.Add('https://hentaicdn.com/hentai' .. v1.ToString())
		end
		return true
	else
		return false
	end
end

function GetNameAndLink()
	if HTTP.GET(MODULE.RootURL..'/directory/newest?page='..(URL + 1)) then
		x=CreateTXQuery(HTTP.Document)
		v=x.XPathHREFAll('//div[contains(@class, "seriesBlock")]/div/div[2]/a', LINKS, NAMES)
		return no_error
	else
		return net_problem
	end
end

function getdirectorypagenumber()
	if HTTP.GET(MODULE.RootURL..'/directory/newest') then
		x = CreateTXQuery(HTTP.Document)
		PAGENUMBER = tonumber(x.XPathString('(//ul[contains(@class,"pagination")]/li/a)[last()-1]')) or 1
		return true
	else
		return false
	end
end

function Init()
	local m = NewWebsiteModule()
	m.ID                       = '66b0b8597daf4839bc7095c10e88039b'
	m.Category                 = 'H-Sites'
	m.Name                     = 'HentaiHere'
	m.RootURL                  = 'https://hentaihere.com'
	m.SortedList               = true
	m.LastUpdated              = 'February 15, 2018'
	m.OnGetInfo                = 'GetInfo'
	m.OnGetPageNumber          = 'GetPageNumber'
	m.OnGetNameAndLink         = 'GetNameAndLink'
	m.OnGetDirectoryPageNumber = 'getdirectorypagenumber'
end