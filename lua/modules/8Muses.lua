function Init()
	local m = NewWebsiteModule()
	m.ID                         = 'd5d532724aba452ea6d4b50663b76326'
	m.Name                       = '8Muses'
	m.RootURL                    = 'https://comics.8muses.com'
	m.OnGetInfo                  = 'GetInfo'
	m.OnGetPageNumber            = 'GetPageNumber'
	m.OnGetImageURL              = 'GetImageURL'
end

function GetInfo()
	MANGAINFO.URL = MaybeFillHost(MODULE.RootURL, URL)
	if HTTP.GET(MANGAINFO.URL) then
		local x = CreateTXQuery(HTTP.Document)

		MANGAINFO.CoverLink = MaybeFillHost(MODULE.RootURL, x.XPathString('//*[@class="gallery"]/a/div/img/@data-src')):gsub('^//', 'https://')
		MANGAINFO.Title     = x.XPathString('//*[@class="top-menu-breadcrumb"]//li[last()]')
		local s = ''
		if MANGAINFO.Title:find('[iI][sS][sS][uU][eE]%s%d') then
			s = MANGAINFO.Title
			MANGAINFO.Title = x.XPathString('//*[@class="top-menu-breadcrumb"]//li[last()-1]')
		end
		-- multi
		local v, n
		for v in x.XPath('//*[@class="gallery"]/a[@href!="" and ./div[@class="image-title"]]').Get() do
			n = v.ToString()
			if (MANGAINFO.Title ~= '') and n:find('[iI][sS][sS][uU][eE]') then
				n = MANGAINFO.Title .. '-' .. n
			end
			MANGAINFO.ChapterLinks.Add(v.GetAttribute('href'))
			MANGAINFO.ChapterNames.Add(n)
		end
		
		if x.XPath('//*[@class="gallery"]/a[@href!="" and not(./div[@class="image-title"])]').Count > 0 then
			MANGAINFO.ChapterLinks.Add(MANGAINFO.URL)
			if s ~= '' then
				MANGAINFO.ChapterNames.Add(MANGAINFO.Title .. ' - ' .. s)
			else
				MANGAINFO.ChapterNames.Add(MANGAINFO.Title)
			end
		end
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL):gsub('/+$', '')) then
		local x = CreateTXQuery(HTTP.Document)
		local v, s
		for v in x.XPath('//*[@class="gallery"]/a[@href!="" and not(./div[@class="image-title"])]/div/img/@data-src').Get() do
			TASK.PageLinks.Add(MaybeFillHost(MODULE.RootURL, v.ToString():gsub('/th/', '/fm/')):gsub('^//', 'https://'))
		end
		if TASK.PageLinks.Count == 0 then
			for v in x.XPath('//*[@class="gallery"]/a[@href!=""]/@href').Get() do
				TASK.PageContainerLinks.Add(v.ToString())
			end
			TASK.PageNumber = TASK.PageContainerLinks.Count
		end
		return true
	else
		return false
	end
end

function GetImageURL()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL, TASK.PageContainerLinks[WORKID]):gsub('/*$', '')) then
		TASK.PageLinks[WORKID] = MaybeFillHost(MODULE.RootURL, CreateTXQuery(HTTP.Document).XPathString('string-join((//*[@id="imageDir"]/@value,//*[@id="imageName"]/@value),"")'))
		return true
	end
		return false
end

